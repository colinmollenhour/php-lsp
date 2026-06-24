<?php

namespace App;

class Noise28
{
    private function compute(): int
    {
        return 28;
    }

    public function process(): int
    {
        return $this->compute() + $this->process();
    }
}
