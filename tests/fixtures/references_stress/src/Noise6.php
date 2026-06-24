<?php

namespace App;

class Noise6
{
    private function compute(): int
    {
        return 6;
    }

    public function process(): int
    {
        return $this->compute() + $this->process();
    }
}
